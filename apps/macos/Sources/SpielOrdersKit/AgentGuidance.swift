import CSpielOrders
import Foundation

public enum AgentGuidance {
    /// Provision independently of any open Package. Cancellation can stop planning,
    /// but a commit always settles and its actual outcome is returned.
    @concurrent public static func provision(in directory: URL, cancellation: LibraryCancellation) async throws -> [String] {
        let path = try FileSystemPath(url: directory)
        var planned: UnsafeMutablePointer<SpielordersPlan>?
        var report: UnsafeMutablePointer<SpielordersReport>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        defer { spielorders_plan_free(planned); spielorders_report_free(report) }
        let result = path.withUnsafeFileSystemRepresentation {
            spielorders_provision_agent($0, cancellation.token, &planned, &outcome)
        }
        var notices: [String]
        do { notices = try checked(result, outcome).map(\.presentation) }
        catch {
            if result == SPIELORDERS_RESULT_CANCELLED { throw CancellationError() }
            throw error
        }
        guard let planned else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        try Task.checkCancellation()
        outcome = nil
        let committed = spielorders_plan_commit(planned, &report, &outcome)
        notices += try checked(committed, outcome).map(\.presentation)
        guard report != nil else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        return notices
    }
}
