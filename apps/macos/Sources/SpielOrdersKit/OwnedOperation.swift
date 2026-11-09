import Foundation

@MainActor public final class OwnedOperation {
    public enum Kind { case read, search, mutation }
    private let kind: Kind
    private let task: Task<Void, Never>

    public init(kind: Kind, operation: @escaping @MainActor () async -> Void) {
        self.kind = kind
        task = Task { await operation() }
    }

    public func settleForExit() async {
        if kind != .mutation { task.cancel() }
        await task.value
    }
}
