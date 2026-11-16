import Foundation

/// Owns a dialog's replanning calls, including superseded calls still leaving the library.
/// The caller keeps its folder access and exit reservation until `finish` returns.
@MainActor public final class Planning {
    private struct Pending {
        let cancel: () -> Void
        let wait: () async -> Void
    }

    private var pending: [UUID: Pending] = [:]
    private var closed = false

    public init() {}

    public func run<Value: Sendable>(
        _ operation: @escaping @Sendable (LibraryCancellation) async throws -> Value
    ) async throws -> Value {
        guard !closed else { throw CancellationError() }
        try Task.checkCancellation()
        let id = UUID()
        let cancellation = LibraryCancellation()
        let task = Task { try await operation(cancellation) }
        pending[id] = Pending(cancel: {
            cancellation.request()
            task.cancel()
        }, wait: { _ = await task.result })
        defer { pending[id] = nil }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            cancellation.request()
            task.cancel()
        }
    }

    /// Stops accepting work and waits even for calls that cannot stop immediately.
    public func finish() async {
        closed = true
        let remaining = Array(pending.values)
        for work in remaining { work.cancel() }
        for work in remaining { await work.wait() }
    }
}
