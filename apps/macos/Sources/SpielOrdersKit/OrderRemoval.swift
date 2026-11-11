import CSpielOrders
import Foundation

struct RemovalOutcome: Sendable {
    let removedDirectory: URL
    let trashFailure: String?
    let warnings: [LibraryEntry]
    let requiresReopen: Bool
    var message: String {
        var text = trashFailure.map {
            "Order removed, but the Trash move failed. It remains recoverable at " + removedDirectory.path + ".\n" + $0
        } ?? "Order moved to Trash."
        if requiresReopen { text += "\nReopen the Package before continuing." }
        if !warnings.isEmpty { text += "\n" + warnings.map(\.presentation).joined(separator: "\n") }
        return text
    }
}

extension PackageSession {
    func refreshPackageAfterCommit(_ previous: LibraryHandle<SpielordersPackage>) -> [LibraryEntry] {
        package = nil
        var refreshed: UnsafeMutablePointer<SpielordersPackage>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        let result = spielorders_package_open(spielorders_package_root(previous.pointer), 0, &refreshed, &outcome)
        let warnings: [LibraryEntry]
        do { warnings = try checked(result, outcome) }
        catch let failure as LibraryFailure { warnings = failure.entries }
        catch { return [] }
        if let refreshed { package = LibraryHandle(refreshed, free: spielorders_package_free) }
        return warnings
    }
}
