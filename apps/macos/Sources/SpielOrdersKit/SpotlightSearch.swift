import CSpielOrders
import Darwin
import Foundation

@MainActor public final class SpotlightSearch {
    private var operation: Task<MaterialBatch, Never>?
    public init() {}

    public func run(directory: URL?, text: String) async -> MaterialBatch {
        let running = Task { await self.perform(directory: directory, text: text) }
        operation = running
        defer { operation = nil }
        return await withTaskCancellationHandler {
            await running.value
        } onCancel: { running.cancel() }
    }

    public func cancel() { operation?.cancel() }

    private func perform(directory: URL?, text: String) async -> MaterialBatch {
        guard let directory else { return MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_COMPLETE) }
        if Task.isCancelled { return MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_CANCELLED) }
        guard await Self.canRead(directory) else {
            return MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED)
        }
        switch await SpotlightIndexState.read(directory) {
        case .enabled: break
        case let .unavailable(state, diagnostic): return MaterialBatch(state: state, diagnostics: [diagnostic])
        }
        return await SpotlightQueryRun(directory: directory, text: text).run()
    }

    @concurrent private static func canRead(_ directory: URL) async -> Bool {
        directory.isFileURL && directory.withUnsafeFileSystemRepresentation({ path in
            guard let path else { return false }
            let descriptor = Darwin.open(path, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
            guard descriptor >= 0 else { return false }
            Darwin.close(descriptor)
            return true
        })
    }
}

@MainActor private final class SpotlightQueryRun {
    private let query = NSMetadataQuery()
    private var observer: NSObjectProtocol?
    private var continuation: CheckedContinuation<MaterialBatch, Never>?

    init(directory: URL, text: String) {
        query.searchScopes = [directory]
        query.predicate = NSPredicate(format: "kMDItemFSName CONTAINS[cd] %@ OR kMDItemTextContent CONTAINS[cd] %@", text, text)
        query.operationQueue = .main
        query.notificationBatchingInterval = 0.1
    }

    func run() async -> MaterialBatch {
        if Task.isCancelled { return MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_CANCELLED) }
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                self.continuation = continuation
                observer = NotificationCenter.default.addObserver(
                    forName: .NSMetadataQueryDidFinishGathering, object: query, queue: .main
                ) { [weak self] _ in
                    Task { @MainActor in self?.finish(SPIELORDERS_PROVIDER_STATE_COMPLETE) }
                }
                guard query.start() else {
                    finish(SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED, diagnostic: "Spotlight refused the query.")
                    return
                }
            }
        } onCancel: {
            Task { @MainActor in
                self.finish(self.query.isGathering ? SPIELORDERS_PROVIDER_STATE_CANCELLED : SPIELORDERS_PROVIDER_STATE_COMPLETE)
            }
        }
    }

    private func finish(_ state: Int32, diagnostic: String? = nil) {
        guard let continuation else { return }
        self.continuation = nil
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil
        query.disableUpdates()
        query.stop()
        let hits = query.results.compactMap { result -> MaterialHit? in
            guard let item = result as? NSMetadataItem else { return nil }
            return MaterialHit(metadataItem: item)
        }
        continuation.resume(returning: MaterialBatch(state: state, hits: hits, diagnostics: diagnostic.map { [$0] } ?? []))
    }
}
