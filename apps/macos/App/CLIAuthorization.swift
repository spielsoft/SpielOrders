import AppKit
import OSLog

/// Launch Services gives the helper its own sandbox. A temporary reply file carries
/// its result; an implicit bookmark shares only that file for this invocation.
@MainActor final class CLIAuthorization {
    private var application: NSRunningApplication?
    private var cancelled = false
    private var termination: NSKeyValueObservation?
    private var settled: CheckedContinuation<Void, Never>?
    private var request: URL?

    func authorize(_ root: URL) async throws -> Bool {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("CLI-Authorization-" + UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer {
            request = nil
            try? FileManager.default.removeItem(at: directory)
        }
        let reply = directory.appendingPathComponent("result.plist")
        try Data().write(to: reply)
        let expected = try root.bookmarkData(options: [.minimalBookmark], includingResourceValuesForKeys: nil, relativeTo: nil)
        let response = try reply.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
        let bundle = Bundle.main.bundleURL.appendingPathComponent(AgentAuthorizationContract.relativeBundle)
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.createsNewApplicationInstance = true
        configuration.allowsRunningApplicationSubstitution = false
        configuration.activates = false
        // Launch Services strips arguments and environment from sandboxed callers.
        // Its document-open handoff conveys access to this disposable request file.
        let request = directory.appendingPathComponent("request.spielorders-authorization")
        try PropertyListEncoder().encode(AgentAuthorizationRequest(packageBookmark: expected, replyBookmark: response))
            .write(to: request)
        self.request = request
        return try await withTaskCancellationHandler {
            try Task.checkCancellation()
            let launched = try await NSWorkspace.shared.open([request], withApplicationAt: bundle, configuration: configuration)
            application = launched
            if cancelled || Task.isCancelled { cancel() }
            await waitForExit(launched)
            application = nil
            try Task.checkCancellation()
            guard let answer = try? PropertyListDecoder().decode(AgentAuthorizationResult.self, from: Data(contentsOf: reply)) else {
                throw AuthorizationFailure(message: "The CLI authorization app ended without reporting a result.")
            }
            switch answer {
            case .authorized: return true
            case .cancelled: return false
            case .failed(let message): throw AuthorizationFailure(message: message)
            }
        } onCancel: {
            Task { @MainActor [weak self] in self?.cancel() }
        }
    }

    private func waitForExit(_ launched: NSRunningApplication) async {
        if launched.isTerminated { return }
        await withCheckedContinuation { continuation in
            settled = continuation
            termination = launched.observe(\.isTerminated, options: [.initial, .new]) { [weak self] _, _ in
                Task { @MainActor in self?.didTerminate() }
            }
        }
        termination = nil
    }

    private func didTerminate() {
        guard application?.isTerminated == true else { return }
        let continuation = settled
        settled = nil
        continuation?.resume()
    }

    private func cancel() {
        cancelled = true
        // The sandbox forbids sending Quit Apple events to the independent helper.
        // Removing our request signals its file watcher without broader access.
        if let request {
            do { try FileManager.default.removeItem(at: request) }
            catch CocoaError.fileNoSuchFile { }
            catch {
                Logger(subsystem: Bundle.main.bundleIdentifier ?? "SpielOrders", category: "authorization")
                    .error("Could not cancel CLI authorization: \(error.localizedDescription, privacy: .public)")
            }
        }
    }
}

private struct AuthorizationFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}
