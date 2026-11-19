import Observation
import SpielOrdersKit
import Testing

// Decision source: Prototype the SpielOrders desktop experience, Order and Vendor editing.

@MainActor @Observable private final class Window {
    var busy = true
}

/// Lets the main actor run until `done` holds, or `limit` has passed.
@MainActor private func settle(_ limit: Duration = .seconds(2), until done: () -> Bool = { false }) async throws {
    let end = ContinuousClock.now + limit
    while !done(), ContinuousClock.now < end { try await Task.sleep(for: .milliseconds(1)) }
}

@Test @MainActor func workAskedForWhileBusyRunsInOrderOnceFree() async throws {
    let window = Window()
    let waiting = WaitingWork { window.busy }
    var ran: [String] = []
    waiting.whenFree {
        ran.append("field")
        window.busy = true
    }
    waiting.whenFree { ran.append("name") }
    try await settle(.milliseconds(50))
    #expect(ran.isEmpty)

    window.busy = false
    try await settle { !ran.isEmpty }
    try await settle(.milliseconds(50))
    #expect(ran == ["field"])

    window.busy = false
    try await settle { ran.count == 2 }
    #expect(ran == ["field", "name"])
}

@Test @MainActor func clearedWorkNeverRuns() async throws {
    let window = Window()
    let waiting = WaitingWork { window.busy }
    var ran: [String] = []
    waiting.whenFree { ran.append("before") }
    waiting.clear()
    waiting.whenFree { ran.append("after") }

    window.busy = false
    try await settle { !ran.isEmpty }
    try await settle(.milliseconds(50))
    #expect(ran == ["after"])
}
