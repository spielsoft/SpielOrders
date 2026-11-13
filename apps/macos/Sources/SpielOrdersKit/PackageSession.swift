import CSpielOrders
import Foundation

struct LibraryEntry: Sendable {
    let message: String
    let code: Int32
    let severity: Int32
    let reason: Int32
    let field: Int32
    let row: Int64
    let path: String
    let evidence: String
    let before: String
    var presentation: String {
        var lines = ["\(String(cString: spielorders_constant_name(severity))): \(String(cString: spielorders_constant_name(code)))", message]
        if !path.isEmpty { lines.append("Location: " + path) }
        if field >= 0 { lines.append("Field: " + String(cString: spielorders_constant_name(field))) }
        if reason >= 0 { lines.append("Reason: " + String(cString: spielorders_constant_name(reason))) }
        if row >= 0 { lines.append("Row: \(row + 1)") }
        if !before.isEmpty { lines.append("Before: " + before) }
        if !evidence.isEmpty { lines.append("Evidence: " + evidence) }
        return lines.joined(separator: "\n")
    }
}

struct LibraryFailure: Error, LocalizedError, Sendable {
    let result: Int32
    let entries: [LibraryEntry]
    var errorDescription: String? {
        (["Result: " + String(cString: spielorders_constant_name(result))] + entries.map(\.presentation)).joined(separator: "\n\n")
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
                     code: spielorders_entries_code(outcome, index), severity: spielorders_entries_severity(outcome, index),
                     reason: spielorders_entries_reason(outcome, index),
                     field: spielorders_entries_field(outcome, index),
                     row: spielorders_entries_row(outcome, index),
                     path: String(cString: spielorders_entries_path(outcome, index)),
                     evidence: String(cString: spielorders_entries_after(outcome, index)),
                     before: String(cString: spielorders_entries_before(outcome, index)))
    }
}

/// Where a list call writes its list, and its entries.
typealias ListOut = UnsafeMutablePointer<UnsafeMutablePointer<SpielordersList>?>
typealias EntriesOut = UnsafeMutablePointer<UnsafeMutablePointer<SpielordersEntries>?>

func checked(_ result: Int32, _ outcome: UnsafeMutablePointer<SpielordersEntries>?) throws -> [LibraryEntry] {
    let entries = consumeEntries(outcome)
    guard result == SPIELORDERS_RESULT_OK else { throw LibraryFailure(result: result, entries: entries) }
    return entries
}

public actor PackageSession {
    var package: LibraryHandle<SpielordersPackage>?
    /// Each README read for editing, by the ID of the Order it belongs to; the Package
    /// README's key is nil. A save is guarded by the bytes its README was read with.
    private var readmes: [String?: LibraryHandle<SpielordersReadme>] = [:]

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
        readmes = [nil: LibraryHandle(original, free: spielorders_readme_free)]
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
        var notices = try checked(result, outcome).map(\.presentation)
        guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        try Task.checkCancellation()
        outcome = nil
        let committed = spielorders_plan_commit(planned, &report, &outcome)
        notices += try checked(committed, outcome).map(\.presentation)
        do {
            // Choice: once committed, refresh cannot turn cancellation into an apparent refusal.
            let text = try load(root, token: 0)
            let available = readmes[nil].map { ReadmeText($0.pointer).readable } ?? false
            let status = available ? "Package created."
                : "Package created. The README is unreadable."
            return PackageInitialization(readmeText: available ? text : nil,
                                         canBrowseRecords: !diagnosticOnly,
                                         message: status, notices: notices)
        } catch {
            notices.append(error.localizedDescription)
            return PackageInitialization(readmeText: nil, canBrowseRecords: false,
                message: "Package created. Reload the Package before continuing.", notices: notices)
        }
    }

    /// Reads a README for editing: `order`'s, or the Package's when `order` is nil.
    public func readme(of order: OrderRow?, cancellation: LibraryCancellation) throws -> ReadmeText {
        let (document, _) = try readReadme(order: order, token: cancellation.token)
        readmes[order?.id] = document
        return ReadmeText(document.pointer)
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

    /// Writes `text` over the README `readme(of:)` last read or this session last saved for
    /// `order`, refusing when that README changed since.
    public func saveReadme(_ text: String, of order: OrderRow?,
                           cancellation: LibraryCancellation) throws -> ReadmeSaveResult {
        guard package != nil, let readme = readmes[order?.id] else { throw SessionAccessError.reopenRequired }
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
        readmes[order?.id] = nil
        do {
            let (refreshed, notices) = try readReadme(order: order, token: 0)
            warnings += notices
            let saved = ReadmeSaveResult(refreshedReadme: refreshed.pointer, warnings: warnings.map(\.presentation))
            if saved.canEdit { readmes[order?.id] = refreshed }
            return saved
        } catch let failure as LibraryFailure {
            warnings += failure.entries
            return ReadmeSaveResult(refreshedText: nil, warnings: warnings.map(\.presentation))
        } catch {
            return ReadmeSaveResult(refreshedText: nil, warnings: warnings.map(\.presentation) + [error.localizedDescription])
        }
    }
}
