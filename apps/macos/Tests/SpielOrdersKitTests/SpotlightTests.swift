import CSpielOrders
import Foundation
@testable import SpielOrdersKit
import Testing

// Decision sources: Effect and adapter boundaries; Desktop acceptance.
@Test @MainActor func aMissingSearchScopeNeverStartsAnUnrestrictedQuery() async {
    let result = await SpotlightSearch().run(directory: nil, text: "quote")
    #expect(result.state == SPIELORDERS_PROVIDER_STATE_COMPLETE)
    #expect(result.hits.isEmpty)
}

@Test @MainActor func anInaccessibleScopeIsNotReportedAsAnEmptySearch() async {
    let absent = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let result = await SpotlightSearch().run(directory: absent, text: "quote")
    #expect(result.state == SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED)
}
