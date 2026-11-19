import Observation

/// Work asked for while the window is busy, run in the order it was asked once the window is
/// free. Each action starts only while the window is free, so one that makes it busy again
/// holds the rest until it frees.
@MainActor public final class WaitingWork {
    private let busy: @MainActor @Sendable () -> Bool
    private var actions: [@MainActor () -> Void] = []
    private var watching: Task<Void, Never>?

    /// `busy` reads observable state, whose changes tell the work when the window frees.
    public init(busy: @escaping @MainActor @Sendable () -> Bool) {
        self.busy = busy
    }

    /// Runs `action` once the window is free, after the actions already waiting; never
    /// within this call.
    public func whenFree(_ action: @escaping @MainActor () -> Void) {
        actions.append(action)
        guard watching == nil else { return }
        watching = Task { [weak self, busy] in
            for await busy in Observations(busy) {
                guard let self else { return }
                if !busy { resume() }
                if actions.isEmpty {
                    watching = nil
                    return
                }
            }
        }
    }

    /// Drops the actions still waiting, such as those asked for in a Package since closed.
    public func clear() {
        actions.removeAll()
    }

    private func resume() {
        while !busy(), !actions.isEmpty { actions.removeFirst()() }
    }
}
