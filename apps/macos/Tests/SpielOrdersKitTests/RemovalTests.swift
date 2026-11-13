import Foundation
@testable import SpielOrdersKit
import Testing

// Decision sources: Effect and adapter boundaries; Desktop acceptance.
@Test func trashRecovery() {
    let removed = FileManager.default.temporaryDirectory.appendingPathComponent("Removed Order")
    let result = RemovalOutcome(removedDirectory: removed, trashFailure: "Permission denied.",
                                warnings: [], requiresReopen: true)
    #expect(result.message.contains("Order removed, but the Trash move failed."))
    #expect(result.message.contains("remains recoverable at " + removed.path))
    #expect(result.message.contains("Permission denied."))
    #expect(result.message.contains("Reload the Package"))
}
