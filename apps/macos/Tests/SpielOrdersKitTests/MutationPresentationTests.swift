import CSpielOrders
import Foundation
@testable import SpielOrdersKit
import Testing

// Decision source: Desktop acceptance, committed mutation presentation.
@Test func missingPackage() async {
    do {
        _ = try await PackageSession().materialDirectory()
        Issue.record("A session without a Package cannot search")
    } catch {
        #expect(error.localizedDescription.contains("reload the Package"))
    }
}

/// No public path leaves a README unreadable between its save and its refresh, so the refresh
/// is read through the C interface and presented as a save would present it.
@Test func unreadableReadme() throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    try Data([0xff]).write(to: root.appendingPathComponent("README.md"))
    var package: UnsafeMutablePointer<SpielordersPackage>?
    var readme: UnsafeMutablePointer<SpielordersReadme>?
    defer { spielorders_readme_free(readme); spielorders_package_free(package) }
    let opened = root.withUnsafeFileSystemRepresentation { spielorders_package_open($0!, 0, &package, nil) }
    #expect(opened == SPIELORDERS_RESULT_OK)
    #expect(spielorders_package_read_readme(try #require(package), 0, &readme, nil) == SPIELORDERS_RESULT_OK)
    let refreshed = try #require(readme)
    #expect(spielorders_readme_state(refreshed) == SPIELORDERS_README_STATE_UNREADABLE)
    let result = ReadmeSaveResult(refreshedReadme: refreshed, warnings: ["The Package could not be refreshed."])
    #expect(result.refreshedText == nil)
    #expect(!result.canEdit)
    #expect(result.message.contains("Saved."))
    #expect(result.message.contains("Reload the Package"))
    #expect(result.message.contains("could not be refreshed"))
}
