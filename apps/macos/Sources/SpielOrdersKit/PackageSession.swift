import CSpielOrders
import Foundation

struct LibraryEntry: Sendable {
    let message: String
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

func checked(_ result: Int32, _ outcome: UnsafeMutablePointer<SpielordersEntries>?) throws -> [LibraryEntry] {
    defer { spielorders_entries_free(outcome) }
    let entries = (0..<spielorders_entries_count(outcome)).map { index in
        LibraryEntry(message: String(cString: spielorders_entries_message(outcome, index)))
    }
    guard result == SPIELORDERS_RESULT_OK else { throw LibraryFailure(result: result, entries: entries) }
    return entries
}

public actor PackageSession {
    var package: LibraryHandle<SpielordersPackage>?
    private var readme: LibraryHandle<SpielordersReadme>?

    public init() {}

    public func open(_ root: URL, cancellation: LibraryCancellation) throws -> String {
        guard root.isFileURL else { throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []) }
        var opened: UnsafeMutablePointer<SpielordersPackage>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = try root.withUnsafeFileSystemRepresentation { path in
            guard let path else { throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []) }
            return spielorders_package_open(path, cancellation.token, &opened, &outcome)
        }
        _ = try checked(result, outcome)
        guard let opened else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let owner = LibraryHandle(opened, free: spielorders_package_free)
        var original: UnsafeMutablePointer<SpielordersReadme>?
        outcome = nil
        let read = spielorders_package_read_readme(opened, cancellation.token, &original, &outcome)
        _ = try checked(read, outcome)
        guard let original else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        package = owner
        readme = LibraryHandle(original, free: spielorders_readme_free)
        return String(cString: spielorders_readme_text(original))
    }

    public var readmeWritable: Bool {
        package != nil && (readme.map { spielorders_readme_state($0.pointer) != SPIELORDERS_README_STATE_UNREADABLE } ?? false)
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
        guard let package else { return ReadmeSaveResult(refreshedText: nil, warnings: warnings.map(\.message)) }
        var refreshed: UnsafeMutablePointer<SpielordersReadme>?
        outcome = nil
        let read = spielorders_package_read_readme(package.pointer, 0, &refreshed, &outcome)
        do {
            warnings += try checked(read, outcome)
        } catch let failure as LibraryFailure {
            warnings += failure.entries
            return ReadmeSaveResult(refreshedText: nil, warnings: warnings.map(\.message))
        }
        guard let refreshed else { return ReadmeSaveResult(refreshedText: nil, warnings: warnings.map(\.message)) }
        let saved = ReadmeSaveResult(refreshedReadme: refreshed, warnings: warnings.map(\.message))
        if saved.canEdit { self.readme = LibraryHandle(refreshed, free: spielorders_readme_free) }
        else { spielorders_readme_free(refreshed) }
        return saved
    }
}
