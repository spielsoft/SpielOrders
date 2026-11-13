import CSpielOrders
import Foundation
import Observation
import SpielOrdersKit

/// Folder Contents' Supporting Material. Spotlight gathers it after a typing pause, outside
/// the window's owned work, so the grid and the detail stay usable while it answers.
@MainActor @Observable final class FolderContents {
    private enum Outcome {
        case answered(MaterialAnswer)
        case refused(Notice)
    }

    /// How long typing pauses before Spotlight is asked.
    private static let pause = Duration.milliseconds(300)

    /// The query being gathered or answered.
    private(set) var query: String?
    private(set) var gathering = false
    private(set) var stopping = false
    /// Why Spotlight may have missed matching files for the query, shown at the search field.
    private(set) var problem: Notice?
    private var answer: MaterialAnswer?
    @ObservationIgnored private var active: Gathering?
    @ObservationIgnored private var pending: [UUID: Task<Void, Never>] = [:]

    /// The library's Supporting Material search for `query`, once Spotlight has answered it.
    func answer(for query: String) -> MaterialAnswer? {
        self.query == query ? answer : nil
    }

    /// Gathers `query` unless it is already being gathered or answered; `answered` follows an
    /// answer the grid can use.
    func gather(_ query: String, session: PackageSession, folder: AuthorizedFolder,
                answered: @escaping @MainActor () -> Void) {
        guard query != self.query else { return }
        stop()
        self.query = query
        gathering = true
        let operation = Gathering()
        active = operation
        let id = UUID()
        pending[id] = Task {
            defer {
                pending[id] = nil
                withExtendedLifetime(folder) {}
            }
            let outcome = await Self.search(query, session: session, operation: operation)
            guard !Task.isCancelled, let outcome else { return }
            gathering = false
            stopping = false
            active = nil
            switch outcome {
            case let .answered(found):
                answer = found
                problem = Self.shortfall(found.state).map {
                    Notice($0, details: found.diagnostics.joined(separator: "\n\n"))
                }
                answered()
            case let .refused(notice):
                problem = notice
            }
        }
    }

    /// Stops the provider without cancelling the operation that preserves its available hits.
    func stopGathering() {
        guard gathering else { return }
        stopping = true
        active?.stop()
    }

    /// Stops any gathering and forgets the answer.
    func stop() {
        for task in pending.values { task.cancel() }
        active = nil
        query = nil
        answer = nil
        problem = nil
        gathering = false
        stopping = false
    }

    /// Exit waits for every superseded provider too, retaining its folder grant until it settles.
    func settle() async {
        stop()
        for task in Array(pending.values) { await task.value }
    }

    /// Nil when cancelled.
    private static func search(_ query: String, session: PackageSession, operation: Gathering) async -> Outcome? {
        var state = SPIELORDERS_PROVIDER_STATE_COMPLETE
        do {
            try await Task.sleep(for: pause)
            let directory = try await session.materialDirectory()
            try Task.checkCancellation()
            let provider = Task { await SpotlightSearch.run(directory: directory, text: query) }
            operation.provider = provider
            if operation.stopped { provider.cancel() }
            let batch = await withTaskCancellationHandler {
                await provider.value
            } onCancel: { provider.cancel() }
            try Task.checkCancellation()
            operation.provider = nil
            state = batch.state
            let cancellation = LibraryCancellation()
            let answer = try await withTaskCancellationHandler {
                try await session.materialSearch(batch, cancellation: cancellation)
            } onCancel: { cancellation.request() }
            return .answered(answer)
        } catch is CancellationError {
            return nil
        } catch {
            let summary = shortfall(state) ?? "Spotlight couldn’t search Folder Contents."
            return .refused(Notice(summary, details: error.localizedDescription))
        }
    }

    /// One provider run, separate from the parent that hands its settled hits to the library.
    @MainActor private final class Gathering {
        var provider: Task<MaterialBatch, Never>?
        private(set) var stopped = false

        func stop() {
            stopped = true
            provider?.cancel()
        }
    }

    /// Why Spotlight may have missed files in `state`, or nil when it looked everywhere.
    private static func shortfall(_ state: Int32) -> String? {
        switch state {
        case SPIELORDERS_PROVIDER_STATE_COMPLETE: nil
        case SPIELORDERS_PROVIDER_STATE_TIMED_OUT: "Spotlight did not finish; some matching files may be missing."
        case SPIELORDERS_PROVIDER_STATE_CANCELLED: "Spotlight stopped early; some matching files may be missing."
        case SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED: "Spotlight indexing is off for this volume, so no files were searched."
        case SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED: "Spotlight has not indexed this volume, so no files were searched."
        case SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED: "Spotlight could not read the Orders folder."
        case SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED: "Spotlight refused the search."
        default: "Spotlight did not search every file."
        }
    }
}
