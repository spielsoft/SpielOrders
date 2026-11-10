import CSpielOrders
import Foundation

struct LibraryEntry: Sendable {
    let message: String
    let code: Int32
    let field: Int32
    let row: Int64
}

struct LibraryFailure: Error, LocalizedError, Sendable {
    let result: Int32
    let entries: [LibraryEntry]
    var errorDescription: String? {
        entries.isEmpty ? String(cString: spielorders_constant_name(result))
            : entries.map(\.message).joined(separator: "\n")
    }
}

public final class LibraryCancellation: @unchecked Sendable {
    let token = spielorders_cancel_new()
    public init() {}
    public func request() { spielorders_cancel_request(token) }
    deinit { spielorders_cancel_release(token) }
}

// Ownership follows the C header. These handles stay private to the session actor;
// the unchecked conformance permits their destruction off that actor as well.
final class LibraryHandle<T>: @unchecked Sendable {
    let pointer: UnsafeMutablePointer<T>
    private let free: (UnsafeMutablePointer<T>?) -> Void
    init(_ pointer: UnsafeMutablePointer<T>, free: @escaping (UnsafeMutablePointer<T>?) -> Void) {
        self.pointer = pointer
        self.free = free
    }
    deinit { free(pointer) }
}

func consumeEntries(_ outcome: UnsafeMutablePointer<SpielordersEntries>?) -> [LibraryEntry] {
    defer { spielorders_entries_free(outcome) }
    return (0..<spielorders_entries_count(outcome)).map { index in
        LibraryEntry(message: String(cString: spielorders_entries_message(outcome, index)),
                     code: spielorders_entries_code(outcome, index), field: spielorders_entries_field(outcome, index),
                     row: spielorders_entries_row(outcome, index))
    }
}

func checked(_ result: Int32, _ outcome: UnsafeMutablePointer<SpielordersEntries>?) throws -> [LibraryEntry] {
    let entries = consumeEntries(outcome)
    guard result == SPIELORDERS_RESULT_OK else { throw LibraryFailure(result: result, entries: entries) }
    return entries
}

public actor PackageSession {
    var package: LibraryHandle<SpielordersPackage>?
    private var readme: LibraryHandle<SpielordersReadme>?
    private var readmeOrder: OrderRow?

    public init() {}

    public func open(_ root: URL, cancellation: LibraryCancellation) throws -> String {
        try load(root, token: cancellation.token)
    }

    private func load(_ root: URL, token: UInt64) throws -> String {
        guard root.isFileURL else { throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []) }
        var opened: UnsafeMutablePointer<SpielordersPackage>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = try root.withUnsafeFileSystemRepresentation { path in
            guard let path else { throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []) }
            return spielorders_package_open(path, token, &opened, &outcome)
        }
        _ = try checked(result, outcome)
        guard let opened else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let owner = LibraryHandle(opened, free: spielorders_package_free)
        var original: UnsafeMutablePointer<SpielordersReadme>?
        outcome = nil
        let read = spielorders_package_read_readme(opened, token, &original, &outcome)
        _ = try checked(read, outcome)
        guard let original else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        package = owner
        readme = LibraryHandle(original, free: spielorders_readme_free)
        readmeOrder = nil
        return String(cString: spielorders_readme_text(original))
    }

    public func initialize(_ root: URL, ordersPath: String, removedPath: String,
                           cancellation: LibraryCancellation) throws -> PackageInitialization {
        let rootPath = try FileSystemPath(url: root)
        var planned: UnsafeMutablePointer<SpielordersPlan>?
        var report: UnsafeMutablePointer<SpielordersReport>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        defer { spielorders_plan_free(planned); spielorders_report_free(report) }
        let result = try rootPath.withUnsafeFileSystemRepresentation { rootBytes in
            try withInterfaceText(ordersPath) { orders in
                try withInterfaceText(removedPath) { removed in
                    spielorders_plan_initialize(rootBytes, ordersPath.isEmpty ? nil : orders,
                                                 removedPath.isEmpty ? nil : removed,
                                                 cancellation.token, &planned, &outcome)
                }
            }
        }
        var notices = try checked(result, outcome).map(\.message)
        guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        try Task.checkCancellation()
        outcome = nil
        let committed = spielorders_plan_commit(planned, &report, &outcome)
        notices += try checked(committed, outcome).map(\.message)
        do {
            // Choice: once committed, refresh cannot turn cancellation into an apparent refusal.
            let text = try load(root, token: 0)
            let available = readmeWritable
            let status = available ? "Package initialized."
                : "Package initialized. Reopen the Package before continuing."
            return PackageInitialization(readmeText: available ? text : nil,
                                         message: ([status] + notices).joined(separator: "\n"))
        } catch {
            notices.append(error.localizedDescription)
            return PackageInitialization(readmeText: nil,
                message: (["Package initialized. Reopen the Package before continuing."] + notices).joined(separator: "\n"))
        }
    }

    public var readmeWritable: Bool {
        package != nil && (readme.map { spielorders_readme_state($0.pointer) != SPIELORDERS_README_STATE_UNREADABLE } ?? false)
    }

    public func editPackageReadme(cancellation: LibraryCancellation) throws -> String {
        try activateReadme(order: nil, token: cancellation.token)
    }

    public func editOrderReadme(_ row: OrderRow, cancellation: LibraryCancellation) throws -> String {
        try activateReadme(order: row, token: cancellation.token)
    }

    private func activateReadme(order: OrderRow?, token: UInt64) throws -> String {
        let (document, _) = try readReadme(order: order, token: token)
        readme = document
        readmeOrder = order
        return String(cString: spielorders_readme_text(document.pointer))
    }

    private func readReadme(order: OrderRow?, token: UInt64) throws -> (LibraryHandle<SpielordersReadme>, [LibraryEntry]) {
        guard let package else { throw SessionAccessError.reopenRequired }
        if let order {
            let (record, notices) = try readOrder(order, token: token)
            defer { withExtendedLifetime(record) {} }
            guard let document = spielorders_order_readme(record.pointer) else {
                throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: [])
            }
            return (LibraryHandle(document, free: spielorders_readme_free), notices)
        }
        var document: UnsafeMutablePointer<SpielordersReadme>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_package_read_readme(package.pointer, token, &document, &outcome)
        let notices = try checked(result, outcome)
        guard let document else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return (LibraryHandle(document, free: spielorders_readme_free), notices)
    }

    public func saveReadme(_ text: String, cancellation: LibraryCancellation) throws -> ReadmeSaveResult {
        guard package != nil, let readme else { throw SessionAccessError.reopenRequired }
        guard !text.utf8.contains(0) else {
            throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: [])
        }
        var planned: UnsafeMutablePointer<SpielordersPlan>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = text.withCString {
            spielorders_readme_plan_write(readme.pointer, $0, cancellation.token, &planned, &outcome)
        }
        var warnings = try checked(result, outcome)
        guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let plan = LibraryHandle(planned, free: spielorders_plan_free)
        try Task.checkCancellation()
        var report: UnsafeMutablePointer<SpielordersReport>?
        outcome = nil
        let committed = spielorders_plan_commit(plan.pointer, &report, &outcome)
        defer { spielorders_report_free(report) }
        warnings += try checked(committed, outcome)
        self.readme = nil
        do {
            let (refreshed, notices) = try readReadme(order: readmeOrder, token: 0)
            warnings += notices
            let saved = ReadmeSaveResult(refreshedReadme: refreshed.pointer, warnings: warnings.map(\.message))
            if saved.canEdit { self.readme = refreshed }
            return saved
        } catch let failure as LibraryFailure {
            warnings += failure.entries
            return ReadmeSaveResult(refreshedText: nil, warnings: warnings.map(\.message))
        } catch {
            return ReadmeSaveResult(refreshedText: nil, warnings: warnings.map(\.message) + [error.localizedDescription])
        }
    }
}
