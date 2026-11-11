import CSpielOrders
import Foundation

public struct DiagnosticFinding: Sendable, Identifiable {
    public let id: Int64
    public let code: String
    public let severity: String
    public let location: String
    public let message: String
    public let evidence: String
}

public struct RepairOption: Sendable, Identifiable {
    public let id: Int64
    public let code: Int32
    public let fields: [Int32]
    public var title: String {
        switch code {
        case SPIELORDERS_REPAIR_ACTION_REPLACE_MANAGED_RECORD: "Replace Managed Record"
        case SPIELORDERS_REPAIR_ACTION_PROMOTE_STAGING_CANDIDATE: "Promote Staging Candidate"
        case SPIELORDERS_REPAIR_ACTION_ASSIGN_EXISTING_VENDOR: "Assign Existing Vendor"
        case SPIELORDERS_REPAIR_ACTION_CREATE_AND_ASSIGN_VENDOR: "Create and Assign Vendor"
        case SPIELORDERS_REPAIR_ACTION_CLEAR_VENDOR_REFERENCE: "Clear Vendor Reference"
        case SPIELORDERS_REPAIR_ACTION_ADOPT_NEW_ORDER_IDENTITY: "Adopt New Order Identity"
        case SPIELORDERS_REPAIR_ACTION_REPAIR_PACKAGE_LOCATOR: "Repair Package Locator"
        default: String(cString: spielorders_constant_name(code))
        }
    }
}

public struct RepairCase: Sendable, Identifiable {
    public let id: Int64
    public let code: String
    public let path: String
    public let actions: [RepairOption]
}

public final class PackageDiagnosis: Sendable {
    let handle: LibraryHandle<SpielordersDiagnosis>
    let source: LibraryHandle<SpielordersPackage>
    public let isValid: Bool
    public let findings: [DiagnosticFinding]
    public let cases: [RepairCase]

    init(_ pointer: UnsafeMutablePointer<SpielordersDiagnosis>, source: LibraryHandle<SpielordersPackage>) {
        handle = LibraryHandle(pointer, free: spielorders_diagnosis_free)
        self.source = source
        isValid = spielorders_diagnosis_is_valid(pointer) != 0
        let entries = spielorders_diagnosis_findings(pointer)
        defer { spielorders_entries_free(entries) }
        findings = (0..<spielorders_entries_count(entries)).map { index in
            let field = spielorders_entries_field(entries, index)
            let row = spielorders_entries_row(entries, index)
            let location = [String(cString: spielorders_entries_path(entries, index)),
                            field < 0 ? "" : String(cString: spielorders_constant_name(field)),
                            row < 0 ? "" : "row \(row + 1)"].filter { !$0.isEmpty }.joined(separator: " · ")
            return DiagnosticFinding(id: index, code: String(cString: spielorders_constant_name(spielorders_entries_code(entries, index))),
                                     severity: String(cString: spielorders_constant_name(spielorders_entries_severity(entries, index))),
                                     location: location, message: String(cString: spielorders_entries_message(entries, index)),
                                     evidence: String(cString: spielorders_entries_after(entries, index)))
        }
        cases = (0..<spielorders_diagnosis_case_count(pointer)).map { index in
            RepairCase(id: index, code: String(cString: spielorders_constant_name(spielorders_diagnosis_case_code(pointer, index))),
                       path: String(cString: spielorders_diagnosis_case_path(pointer, index)),
                       actions: (0..<spielorders_diagnosis_action_count(pointer, index)).map { action in
                RepairOption(id: action, code: spielorders_diagnosis_action(pointer, index, action),
                             fields: (0..<spielorders_diagnosis_action_field_count(pointer, index, action)).map {
                    spielorders_diagnosis_action_field(pointer, index, action, $0)
                })
            })
        }
    }

    func initialValues(repair: RepairCase, action: RepairOption) throws -> [Int32: String] {
        guard !action.fields.isEmpty else { return [:] }
        let draft = try makeDraft(repair: repair, action: action)
        return withExtendedLifetime(draft) {
            Dictionary(uniqueKeysWithValues: action.fields.map {
                ($0, String(cString: spielorders_draft_field(draft.pointer, $0)))
            })
        }
    }

    func makeDraft(repair: RepairCase, action: RepairOption) throws -> LibraryHandle<SpielordersDraft> {
        var draft: UnsafeMutablePointer<SpielordersDraft>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        _ = try checked(spielorders_diagnosis_draft(handle.pointer, repair.id, action.id, &draft, &outcome), outcome)
        guard let draft else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return LibraryHandle(draft, free: spielorders_draft_free)
    }
}

extension PackageSession {
    public func repairValues(_ diagnosis: PackageDiagnosis, repair: RepairCase, action: RepairOption) throws -> [Int32: String] {
        guard package === diagnosis.source else { throw SessionAccessError.reopenRequired }
        return try diagnosis.initialValues(repair: repair, action: action)
    }

    public func diagnose(directory: URL?, cancellation: LibraryCancellation) throws -> PackageDiagnosis {
        guard let package else { throw SessionAccessError.reopenRequired }
        var diagnosis: UnsafeMutablePointer<SpielordersDiagnosis>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result: Int32
        if let directory {
            result = try FileSystemPath(url: directory).withUnsafeFileSystemRepresentation {
                spielorders_package_diagnose_order(package.pointer, $0, cancellation.token, &diagnosis, &outcome)
            }
        } else {
            result = spielorders_package_diagnose(package.pointer, cancellation.token, &diagnosis, &outcome)
        }
        _ = try checked(result, outcome)
        guard let diagnosis else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return PackageDiagnosis(diagnosis, source: package)
    }

    public func planRepair(_ diagnosis: PackageDiagnosis, repair: RepairCase, action: RepairOption,
                           values: [Int32: String], cancellation: LibraryCancellation) throws -> PackageChange {
        guard let package, package === diagnosis.source else { throw SessionAccessError.reopenRequired }
        let draft = action.fields.isEmpty ? nil : try diagnosis.makeDraft(repair: repair, action: action)
        if let draft {
            for field in action.fields {
                var outcome: UnsafeMutablePointer<SpielordersEntries>?
                let result = try withInterfaceText(values[field] ?? "") {
                    spielorders_draft_set(draft.pointer, field, $0, &outcome)
                }
                _ = try checked(result, outcome)
            }
        }
        var plan: UnsafeMutablePointer<SpielordersPlan>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_diagnosis_plan_repair(diagnosis.handle.pointer, repair.id, action.id,
                                                       draft?.pointer, cancellation.token, &plan, &outcome)
        let notices = try checked(result, outcome)
        guard let plan else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return PackageChange(plan, source: package, notices: notices, action: nil)
    }
}
