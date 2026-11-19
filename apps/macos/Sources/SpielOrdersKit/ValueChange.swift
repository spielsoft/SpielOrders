import CSpielOrders
import Foundation

/// Whether these spellings name one Package value, as the library compares values.
public func valueMatches(_ left: String, _ right: String) -> Bool {
    (try? withInterfaceText(left) { left in
        try withInterfaceText(right) { right in spielorders_value_matches(left, right) != 0 }
    }) ?? false
}

/// Which values a change names: the Orderers, the Ordering Institutions, the Projects, or the
/// Order Groups of one Project.
public struct ValueScope: Sendable, Hashable {
    /// The `SPIELORDERS_ORDER_FIELD_*` constant the values belong to.
    public let field: Int32
    /// The Project whose Order Groups these are, "" for the Orders with no Project; nil for the
    /// other fields.
    public let project: String?

    public static let orderers = ValueScope(field: SPIELORDERS_ORDER_FIELD_ORDERER, project: nil)
    public static let institutions = ValueScope(field: SPIELORDERS_ORDER_FIELD_ORDERING_INSTITUTION, project: nil)
    public static let projects = ValueScope(field: SPIELORDERS_ORDER_FIELD_PROJECT, project: nil)

    /// The Order Groups of `project`, or of the Orders with no Project for "".
    public static func groups(of project: String) -> ValueScope {
        ValueScope(field: SPIELORDERS_ORDER_FIELD_ORDER_GROUP, project: project)
    }

    /// A scope retains the exact Project the library named; Swift's canonical Unicode
    /// equivalence must not combine two distinct library-returned choices.
    public static func == (left: Self, right: Self) -> Bool {
        left.field == right.field && left.project.map { Array($0.utf8) } == right.project.map { Array($0.utf8) }
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(field)
        hasher.combine(project.map { Array($0.utf8) })
    }
}

/// What each Order using a deleted value or Vendor uses instead.
public enum Replacement: Sendable, Hashable {
    /// Refuse while any Order uses it.
    case refuse
    /// None: each such Order leaves the field empty.
    case clear
    /// Another value the Package has, or for a Vendor another Vendor's ID.
    case with(String)

    public static func == (left: Self, right: Self) -> Bool {
        switch (left, right) {
        case (.refuse, .refuse), (.clear, .clear): true
        case let (.with(left), .with(right)): left.utf8.elementsEqual(right.utf8)
        default: false
        }
    }

    public func hash(into hasher: inout Hasher) {
        switch self {
        case .refuse: hasher.combine(0)
        case .clear: hasher.combine(1)
        case let .with(value):
            hasher.combine(2)
            hasher.combine(Array(value.utf8))
        }
    }
}

/// A library-planned change to the Package's values or Vendors. Holding it writes nothing;
/// committing checks that nothing it was planned against has changed.
public final class ValueChange: Sendable {
    let handle: LibraryHandle<SpielordersPlan>
    let source: LibraryHandle<SpielordersPackage>
    let notices: [LibraryEntry]
    /// How many Orders committing the change rewrites.
    public let orderCount: Int

    init(_ pointer: UnsafeMutablePointer<SpielordersPlan>, source: LibraryHandle<SpielordersPackage>,
         notices: [LibraryEntry]) {
        handle = LibraryHandle(pointer, free: spielorders_plan_free)
        self.source = source
        self.notices = notices
        orderCount = Int(spielorders_plan_order_count(pointer))
    }
}

/// What committing a value change left behind.
public struct CommittedValueChange: Sendable {
    /// Whether anything was written: adding a value the Package already has writes nothing.
    public let changed: Bool
    public let warnings: [String]
    /// Whether the Package could not be read again after the commit.
    public let requiresReopen: Bool
}

/// A value change the library refused or could not complete. Details retain any incomplete
/// rollback rather than promising that a failed commit wrote nothing.
public struct ValueRefusal: Error, LocalizedError, Sendable {
    /// Why, in one sentence for the owner.
    public let summary: String
    /// Whether Orders still use what a deletion takes away, so that it needs a replacement.
    public let inUse: Bool
    private let failure: LibraryFailure

    public var errorDescription: String? { failure.errorDescription }

    init(_ failure: LibraryFailure) {
        self.failure = failure
        let codes = Set(failure.entries.map(\.code))
        inUse = !codes.isDisjoint(with: [SPIELORDERS_FAILURE_VALUE_IN_USE, SPIELORDERS_FAILURE_VENDOR_REFERENCED])
        summary = if inUse {
            "Orders use it."
        } else if codes.contains(SPIELORDERS_FAILURE_BLANK_VALUE) {
            "A name is required."
        } else if codes.contains(SPIELORDERS_FAILURE_UNKNOWN_VALUE) {
            "The Package no longer has this value. Reload the Package, then try again."
        } else if codes.contains(SPIELORDERS_FAILURE_UNKNOWN_VENDOR_ID) {
            "The Package no longer has this Vendor. Reload the Package, then try again."
        } else if !codes.isDisjoint(with: [SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED,
                                           SPIELORDERS_FAILURE_PLAN_EVIDENCE_UNCONFIRMED]) {
            "The Package changed. Reload the Package, then try again."
        } else {
            failure.entries.first?.message ?? "The change couldn’t be completed. Details has the reason."
        }
    }
}

/// Where a plan call writes its plan.
private typealias PlanOut = UnsafeMutablePointer<UnsafeMutablePointer<SpielordersPlan>?>

extension PackageSession {
    /// Plans keeping `value` among the values of `scope`, whether or not an Order uses it.
    public func planAdding(_ value: String, to scope: ValueScope,
                           cancellation: LibraryCancellation) throws -> ValueChange {
        try planValueChange { package, plan, outcome in
            try withScope(scope) { project in
                try withInterfaceText(value) { value in
                    spielorders_package_plan_add_value(package, scope.field, project, value, cancellation.token,
                                                       plan, outcome)
                }
            }
        }
    }

    /// Plans respelling `value` as `name` in every Order that uses it. Renaming onto another
    /// value merges the two.
    public func planRenaming(_ value: String, to name: String, in scope: ValueScope,
                             cancellation: LibraryCancellation) throws -> ValueChange {
        try planValueChange { package, plan, outcome in
            try withScope(scope) { project in
                try withInterfaceText(value) { value in
                    try withInterfaceText(name) { name in
                        spielorders_package_plan_rename_value(package, scope.field, project, value, name,
                                                              cancellation.token, plan, outcome)
                    }
                }
            }
        }
    }

    /// Plans making `values` one value spelled `kept`, which every Order using one of them uses.
    public func planMerging(_ values: [String], keeping kept: String, in scope: ValueScope,
                            cancellation: LibraryCancellation) throws -> ValueChange {
        try planValueChange { package, plan, outcome in
            try withScope(scope) { project in
                try withInterfaceTexts(values) { texts in
                    try withInterfaceText(kept) { kept in
                        spielorders_package_plan_merge_values(package, scope.field, project, texts.baseAddress,
                                                              Int64(texts.count), kept, cancellation.token,
                                                              plan, outcome)
                    }
                }
            }
        }
    }

    /// Plans taking `value` out of `scope`; `replacement` says what the Orders using it use instead.
    public func planDeleting(_ value: String, in scope: ValueScope, replacement: Replacement,
                             cancellation: LibraryCancellation) throws -> ValueChange {
        try planValueChange { package, plan, outcome in
            try withScope(scope) { project in
                try withInterfaceText(value) { value in
                    try withReplacement(replacement) { replacement in
                        spielorders_package_plan_delete_value(package, scope.field, project, value, replacement,
                                                              cancellation.token, plan, outcome)
                    }
                }
            }
        }
    }

    /// Whether `name` is another value of `scope` than `value`, so that renaming `value` to it
    /// merges the two. The library answers: it replaces `value` only with another value it has.
    public func renamingMerges(_ value: String, into name: String, in scope: ValueScope) -> Bool {
        (try? planDeleting(value, in: scope, replacement: .with(name), cancellation: LibraryCancellation())) != nil
    }

    /// Plans merging the Vendors `ids` in precedence order: the first is kept, each later one
    /// only fills the facts and Contacts it still lacks, and every Order referencing one of
    /// them references it.
    public func planMergingVendors(_ ids: [String], cancellation: LibraryCancellation) throws -> ValueChange {
        guard let kept = ids.first else {
            throw ValueRefusal(LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []))
        }
        return try planValueChange { package, plan, outcome in
            try withInterfaceTexts(ids) { texts in
                try withInterfaceText(kept) { kept in
                    spielorders_package_plan_merge_vendors(package, texts.baseAddress, Int64(texts.count), kept,
                                                           cancellation.token, plan, outcome)
                }
            }
        }
    }

    /// Plans deleting the Vendor `id`; `replacement` names another Vendor by ID or clears the
    /// Orders referencing it.
    public func planDeletingVendor(_ id: String, replacement: Replacement,
                                   cancellation: LibraryCancellation) throws -> ValueChange {
        do {
            let (vendor, read) = try readVendor(id: id, token: cancellation.token)
            return try planValueChange(read) { _, plan, outcome in
                try withReplacement(replacement) { replacement in
                    spielorders_vendor_plan_delete_replacing(vendor.pointer, replacement, cancellation.token,
                                                             plan, outcome)
                }
            }
        } catch let failure as LibraryFailure {
            throw ValueRefusal(failure)
        }
    }

    /// The ID of the Vendor the Package names `name`, compared as the Package compares Names.
    public func vendorID(named name: String) throws -> String {
        guard let package else { throw SessionAccessError.reopenRequired }
        var vendor: UnsafeMutablePointer<SpielordersVendor>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = try withInterfaceText(name) {
            spielorders_package_read_vendor(package.pointer, SPIELORDERS_VENDOR_SELECTOR_NAME, $0, 0, &vendor, &outcome)
        }
        _ = try checked(result, outcome)
        guard let vendor else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        defer { spielorders_vendor_free(vendor) }
        return String(cString: spielorders_vendor_field(vendor, SPIELORDERS_VENDOR_FIELD_VENDOR_ID))
    }

    /// Commits `change`, then reads the Package again. A refusal throws `ValueRefusal`.
    public func commit(_ change: ValueChange) throws -> CommittedValueChange {
        guard package === change.source else { throw SessionAccessError.reopenRequired }
        try Task.checkCancellation()
        var report: UnsafeMutablePointer<SpielordersReport>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_plan_commit(change.handle.pointer, &report, &outcome)
        defer { spielorders_report_free(report) }
        var warnings = change.notices
        do {
            warnings += try checked(result, outcome)
        } catch let failure as LibraryFailure {
            throw ValueRefusal(failure)
        }
        guard let report else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let changed = spielorders_report_kind(report) == SPIELORDERS_REPORT_COMMITTED
        // An unchanged plan may have found values another writer added since this session's
        // snapshot. Its successful answer must update the choices the caller will list.
        warnings += refreshAfterCommit(change.source, report: report)
        return CommittedValueChange(changed: changed, warnings: warnings.map(\.presentation),
                                    requiresReopen: package == nil)
    }

    private func planValueChange(
        _ read: [LibraryEntry] = [],
        _ planning: (UnsafeMutablePointer<SpielordersPackage>, PlanOut, EntriesOut) throws -> Int32
    ) throws -> ValueChange {
        guard let package else { throw SessionAccessError.reopenRequired }
        var plan: UnsafeMutablePointer<SpielordersPlan>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        do {
            let result = try planning(package.pointer, &plan, &outcome)
            let notices = try read + checked(result, outcome)
            guard let plan else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
            return ValueChange(plan, source: package, notices: notices)
        } catch let failure as LibraryFailure {
            throw ValueRefusal(failure)
        }
    }
}

/// `scope`'s Project as the interface takes it: NULL for a field without Projects.
func withScope<T>(_ scope: ValueScope, _ body: (UnsafePointer<CChar>?) throws -> T) throws -> T {
    guard let project = scope.project else { return try body(nil) }
    return try withInterfaceText(project) { try body($0) }
}

/// `replacement` as the interface takes it: NULL to refuse, "" for none, or the replacement.
private func withReplacement<T>(_ replacement: Replacement, _ body: (UnsafePointer<CChar>?) throws -> T) throws -> T {
    switch replacement {
    case .refuse: try body(nil)
    case .clear: try withInterfaceText("") { try body($0) }
    case let .with(value): try withInterfaceText(value) { try body($0) }
    }
}

/// `texts` as an array of interface texts, valid only within `body`.
private func withInterfaceTexts<T>(_ texts: [String],
                                   _ body: (UnsafeMutableBufferPointer<UnsafePointer<CChar>>) throws -> T) throws -> T {
    guard !texts.contains(where: { $0.utf8.contains(0) }) else {
        throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: [])
    }
    let copies = texts.compactMap { strdup($0) }
    defer { copies.forEach { free($0) } }
    guard copies.count == texts.count else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
    var pointers = copies.map { UnsafePointer($0) }
    return try pointers.withUnsafeMutableBufferPointer { try body($0) }
}
