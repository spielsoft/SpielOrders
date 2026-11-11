import CSpielOrders
import Foundation

public struct AgentGuidance: Sendable {
    public let message: String
    public let prompt: String
}

extension PackageSession {
    public func provisionAgentGuidance(cancellation: LibraryCancellation) throws -> AgentGuidance {
        guard let package else { throw SessionAccessError.reopenRequired }
        var planned: UnsafeMutablePointer<SpielordersPlan>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_package_plan_provision_agent_setup(package.pointer, cancellation.token, &planned, &outcome)
        var notices = try checked(result, outcome).map(\.presentation)
        guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let plan = LibraryHandle(planned, free: spielorders_plan_free)
        try Task.checkCancellation()
        var report: UnsafeMutablePointer<SpielordersReport>?
        outcome = nil
        let committed = spielorders_plan_commit(plan.pointer, &report, &outcome)
        defer { spielorders_report_free(report) }
        notices += try checked(committed, outcome).map(\.presentation)
        guard let report else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let message = spielorders_report_kind(report) == SPIELORDERS_REPORT_UNCHANGED
            ? "Agent guidance already exists; existing files are preserved."
            : "Missing agent guidance created; existing files are preserved."
        return AgentGuidance(message: ([message] + notices).joined(separator: "\n"),
                             prompt: String(cString: spielorders_library_agent_setup_prompt()))
    }
}
