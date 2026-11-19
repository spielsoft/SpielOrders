import CSpielOrders
import Foundation

/// One entry of a later Order's directory that a merge moves or leaves behind.
public struct MergedEntry: Sendable, Hashable, Identifiable {
    /// The ID of the Order whose directory holds the entry.
    public let order: String
    /// Where the entry is within that directory.
    public let path: String
    /// Whether the entry is a folder rather than a file.
    public let isFolder: Bool

    public var id: String { order + "/" + path }
}

/// Orders merged in precedence order, as the library planned it: the first is kept, and each
/// later one only fills what the result still lacks. Holding it writes nothing; committing
/// checks that nothing it was planned against has changed.
public final class OrderMerge: Sendable {
    let handle: LibraryHandle<SpielordersPlan>
    let source: LibraryHandle<SpielordersPackage>
    let notices: [LibraryEntry]
    /// The Orders in precedence order; the first is kept.
    public let orders: [OrderRow]
    /// Each entry of a later Order that moves into the kept Order at the same path.
    public let moved: [MergedEntry]
    /// Each entry of a later Order that stays behind, because the kept Order already holds
    /// its path, and goes with that Order to Removed Orders.
    public let discarded: [MergedEntry]
    /// Whether the kept Order's record gains facts or Items from the others.
    public let fillsKept: Bool

    /// Throws `OrderMergeRefusal` for a plan that moves or discards an entry it can't place in
    /// a later Order, which the owner could then neither see nor confirm.
    init(_ pointer: UnsafeMutablePointer<SpielordersPlan>, orders: [OrderRow],
         source: LibraryHandle<SpielordersPackage>, notices: [LibraryEntry]) throws {
        handle = LibraryHandle(pointer, free: spielorders_plan_free)
        self.source = source
        self.notices = notices
        self.orders = orders
        fillsKept = spielorders_plan_order_count(pointer) > 0
        var moved: [MergedEntry] = []
        var discarded: [MergedEntry] = []
        for index in 0..<spielorders_plan_operation_count(pointer) {
            let kind = spielorders_plan_operation_kind(pointer, index)
            guard kind == SPIELORDERS_OPERATION_MOVE_MERGED_ENTRY
                    || kind == SPIELORDERS_OPERATION_DISCARD_MERGED_ENTRY else { continue }
            let path = bytes(spielorders_plan_operation_path(pointer, index))
            let isFolder = spielorders_plan_operation_directory(pointer, index) == 1
            guard let entry = mergedEntry(path, isFolder: isFolder, among: orders.dropFirst()) else {
                let operation = String(cString: spielorders_constant_name(kind))
                throw OrderMergeRefusal(
                    summary: "The merge names a file or folder outside the Orders it merges, so it can’t be "
                        + "reviewed. Nothing was merged.",
                    details: operation + ": " + String(decoding: path, as: UTF8.self))
            }
            if kind == SPIELORDERS_OPERATION_MOVE_MERGED_ENTRY {
                moved.append(entry)
            } else {
                discarded.append(entry)
            }
        }
        self.moved = moved
        self.discarded = discarded
    }
}

/// The entry at `path`, named by the merged Order whose directory holds it.
private func mergedEntry(_ path: [UInt8], isFolder: Bool, among orders: ArraySlice<OrderRow>) -> MergedEntry? {
    for order in orders {
        let prefix = order.path.withUnsafeFileSystemRepresentation(bytes) + Array("/".utf8)
        guard path.starts(with: prefix) else { continue }
        let relative = String(decoding: path.dropFirst(prefix.count), as: UTF8.self)
        return MergedEntry(order: order.id, path: relative, isFolder: isFolder)
    }
    return nil
}

/// What committing an Order merge left behind.
public struct CommittedOrderMerge: Sendable {
    /// How many merged Orders gave up their entries but could not be moved to Removed Orders.
    public let unremoved: Int
    public let warnings: [String]
    /// Whether the Package could not be read again after the commit.
    public let requiresReopen: Bool
}

/// An Order merge the library refused or could not complete. Details retain any incomplete
/// rollback rather than promising that a failed commit wrote nothing.
public struct OrderMergeRefusal: Error, LocalizedError, Sendable {
    /// Why, in one sentence for the owner.
    public let summary: String
    private let details: String

    public var errorDescription: String? { details }

    init(summary: String, details: String) {
        self.summary = summary
        self.details = details
    }
}

extension PackageSession {
    /// Plans merging `orders` in precedence order: the first is kept, and each later one only
    /// fills what the result still lacks.
    public func planMergingOrders(_ orders: [OrderRow], cancellation: LibraryCancellation) throws -> OrderMerge {
        guard let package else { throw SessionAccessError.reopenRequired }
        guard let first = orders.first else {
            throw refusal(LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []), merging: orders)
        }
        do {
            let (kept, read) = try readOrder(first, token: cancellation.token)
            var plan: UnsafeMutablePointer<SpielordersPlan>?
            var outcome: UnsafeMutablePointer<SpielordersEntries>?
            let result = withPathBytes(orders.dropFirst().map(\.path)) { paths in
                spielorders_order_plan_merge(kept.pointer, paths.baseAddress, Int64(paths.count), cancellation.token,
                                             &plan, &outcome)
            }
            let notices = try read + checked(result, outcome)
            guard let plan else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
            return try OrderMerge(plan, orders: orders, source: package, notices: notices)
        } catch let failure as LibraryFailure {
            throw refusal(failure, merging: orders)
        }
    }

    /// Commits `merge`, then reads the Package again. A refusal throws `OrderMergeRefusal`.
    public func commit(_ merge: OrderMerge) throws -> CommittedOrderMerge {
        guard package === merge.source else { throw SessionAccessError.reopenRequired }
        try Task.checkCancellation()
        var report: UnsafeMutablePointer<SpielordersReport>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_plan_commit(merge.handle.pointer, &report, &outcome)
        defer { spielorders_report_free(report) }
        var warnings = merge.notices
        do {
            warnings += try checked(result, outcome)
        } catch let failure as LibraryFailure {
            throw refusal(failure, merging: merge.orders)
        }
        guard report != nil else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let unremoved = warnings.filter { $0.code == SPIELORDERS_WARNING_ORDER_NOT_REMOVED }.count
        warnings += refreshAfterCommit(merge.source, report: report)
        return CommittedOrderMerge(unremoved: unremoved, warnings: warnings.map(\.presentation),
                                   requiresReopen: package == nil)
    }
}

extension PackageSession {
    /// `failure` in one sentence for the owner, naming the Orders it concerns where it can.
    private func refusal(_ failure: LibraryFailure, merging orders: [OrderRow]) -> OrderMergeRefusal {
        let codes = Set(failure.entries.map(\.code))
        // Only healthy listed Orders are merged, so one that is gone or damaged changed since.
        let stale: Set = [SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED, SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED,
                          SPIELORDERS_FAILURE_NOT_AN_ORDER_DIRECTORY, SPIELORDERS_FAILURE_DAMAGED_RECORD,
                          SPIELORDERS_FAILURE_ORDER_NOT_FOUND, SPIELORDERS_FAILURE_DAMAGED_ORDER,
                          SPIELORDERS_FAILURE_LOCATOR_UNUSABLE,
                          SPIELORDERS_FAILURE_LOCATOR_REACHES_NO_PACKAGE_HOLDING_THIS_ORDER]
        let currency = failure.entries.first { $0.code == SPIELORDERS_FAILURE_CURRENCY_DIFFERS }
        let location = currency.flatMap(located)
        let priced = location.flatMap { location in
            orders.dropFirst().first { location.starts(with: $0.directory.standardizedFileURL.pathComponents) }
        }
        let summary = if !codes.isDisjoint(with: stale) {
            "The Orders changed after the merge was shown. Nothing was merged; review the merge again."
        } else if let priced, let kept = orders.first {
            "“\(priced.name)” prices its Items in another currency, so it can’t be merged into “\(kept.name)”."
        } else {
            failure.entries.first?.message ?? "The merge couldn’t be completed. Details has the reason."
        }
        return OrderMergeRefusal(summary: summary, details: failure.errorDescription ?? "")
    }
}

/// `paths` as an array of interface path bytes, valid only within `body`.
private func withPathBytes<T>(_ paths: [FileSystemPath],
                              _ body: (UnsafeMutableBufferPointer<UnsafePointer<CChar>>) throws -> T) rethrows -> T {
    let copies = paths.map { path in path.withUnsafeFileSystemRepresentation { strdup($0)! } }
    defer { copies.forEach { free($0) } }
    var pointers = copies.map { UnsafePointer($0) }
    return try pointers.withUnsafeMutableBufferPointer { try body($0) }
}

/// The bytes of the path `pointer` names, without its terminator.
private func bytes(_ pointer: UnsafePointer<CChar>) -> [UInt8] {
    UnsafeRawBufferPointer(start: pointer, count: strlen(pointer)).map { $0 }
}
