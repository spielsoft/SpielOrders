import CSpielOrders
import Foundation
@testable import SpielOrdersKit
import Testing

// Decision sources: Effect and adapter boundaries; Desktop acceptance.
@Test @MainActor func missingScope() async {
    let result = await SpotlightSearch.run(directory: nil, text: "quote")
    #expect(result.state == SPIELORDERS_PROVIDER_STATE_COMPLETE)
    #expect(result.hits.isEmpty)
}

@Test @MainActor func deniedScope() async {
    let absent = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let result = await SpotlightSearch.run(directory: absent, text: "quote")
    #expect(result.state == SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED)
}
@MainActor @Test func earlyStop() async {
    let directory = FileManager.default.temporaryDirectory
    let search = Task { @MainActor in
        withUnsafeCurrentTask { $0?.cancel() }
        return await SpotlightSearch.run(directory: directory, text: "quote")
    }
    let answer = await search.value
    #expect(answer.state == SPIELORDERS_PROVIDER_STATE_CANCELLED)
    #expect(answer.hits.isEmpty)
}
