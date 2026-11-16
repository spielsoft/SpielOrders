import Foundation
import SpielOrdersKit
import Testing

// Decision source: Desktop acceptance, application lifecycle.
@Test @MainActor func saveSettlement() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let operation = OwnedOperation(kind: .mutation) {
        do {
            try await Task.sleep(for: .milliseconds(20))
            _ = try await session.saveReadme("Saved before exit", of: nil, cancellation: LibraryCancellation())
        } catch { Issue.record("Exit interrupted an approved Save: \(error)") }
    }
    await operation.settleForExit()
    #expect(try await PackageSession().open(root, cancellation: LibraryCancellation()) == "Saved before exit")
}

@Test @MainActor func waitingForReadDoesNotCancelIt() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    var restored: String?
    let operation = OwnedOperation(kind: .read) {
        do {
            await Task.yield()
            try Task.checkCancellation()
            restored = try await PackageSession().open(root, cancellation: LibraryCancellation())
        } catch { Issue.record("Waiting cancelled a live Package read: \(error)") }
    }
    await operation.wait()
    #expect(restored != nil)
}

/// A library call may finish reading after cancellation was requested.
@MainActor private final class SuspendedRead {
    private var continuation: CheckedContinuation<Void, Never>?
    let events = AsyncStream<String>.makeStream()

    func run() async {
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                self.continuation = continuation
                events.continuation.yield("started")
            }
        } onCancel: { [events] in
            events.continuation.yield("cancelled")
        }
    }

    func resume() {
        continuation?.resume()
        continuation = nil
    }
}

@Test @MainActor func planningSettlement() async throws {
    let planning = Planning()
    let earlier = SuspendedRead()
    var firstEvents = earlier.events.stream.makeAsyncIterator()
    let first = Task { try await planning.run { _ in await earlier.run() } }
    #expect(await firstEvents.next() == "started")
    first.cancel()
    #expect(await firstEvents.next() == "cancelled")

    let latest = SuspendedRead()
    var lastEvents = latest.events.stream.makeAsyncIterator()
    let last = Task { try await planning.run { _ in await latest.run() } }
    #expect(await lastEvents.next() == "started")
    var exited = false
    let operation = OwnedOperation(kind: .mutation) {
        await planning.finish()
        exited = true
    }
    #expect(await lastEvents.next() == "cancelled")
    #expect(!exited, "Exit must wait for the library calls to finish")
    latest.resume()
    _ = await last.result
    #expect(!exited, "Superseded calls still own their folder access")
    earlier.resume()
    _ = await first.result
    await operation.settleForExit()
    #expect(exited)

    await #expect(throws: CancellationError.self) {
        try await planning.run { _ in
            Issue.record("A dismissed dialog started new library work")
        }
    }
}
