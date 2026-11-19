import CSpielOrders
import Foundation

/// A finding or warning the library reported, located where it occurs: in plain words for
/// the place that shows it, with the library's own wording kept for Details.
public struct Problem: Sendable {
    /// The problem in plain words, such as “This Order’s file can’t be read.”
    public let statement: String
    /// The library's own wording: its severity, code, message, and location.
    public let details: String
    /// The Order Directory the library locates it in, or nil for one located elsewhere in
    /// the Package.
    public let order: URL?

    init(_ entry: LibraryEntry, order: URL?) {
        statement = entry.statement(inOrder: order != nil)
        details = entry.presentation
        self.order = order
    }
}

extension PackageSession {
    /// `entries` as problems, each in the Order Directory that holds its location.
    func problems(_ entries: [LibraryEntry], in package: LibraryHandle<SpielordersPackage>) -> [Problem] {
        let directories = entries.contains { !$0.path.isEmpty } ? orderDirectories(in: package) : []
        return entries.map { entry in
            let location = located(entry, in: package)
            let order = location.flatMap { path in directories.first { path.starts(with: $0.components) } }
            return Problem(entry, order: order?.url)
        }
    }

    /// The Order Directories the Package's own findings are located in, each once, in the
    /// order of its findings.
    func affectedOrders(in package: LibraryHandle<SpielordersPackage>) -> [URL] {
        var seen = Set<URL>()
        return problems(consumeEntries(spielorders_package_findings(package.pointer)), in: package).compactMap(\.order)
            .filter { seen.insert($0).inserted }
    }

    /// Every Order Directory the Package lists, damaged ones included; none for a Damaged
    /// Package, whose Orders are not listed.
    private func orderDirectories(in package: LibraryHandle<SpielordersPackage>) -> [(components: [String], url: URL)] {
        var list: UnsafeMutablePointer<SpielordersList>?
        defer { spielorders_list_free(list) }
        guard spielorders_package_orders(package.pointer, nil, 0, -1, 0, 0, &list, nil) == SPIELORDERS_RESULT_OK,
              let list else { return [] }
        return (0..<spielorders_list_count(list)).map { index in
            let url = FileSystemPath(fileSystemRepresentation: spielorders_list_path(list, index)).url
            return (url.standardizedFileURL.pathComponents, url)
        }
    }

    /// The path components of the location `entry` names: Package-relative, or absolute
    /// where it has no Package-relative spelling.
    func located(_ entry: LibraryEntry) -> [String]? {
        guard let package else { return nil }
        return located(entry, in: package)
    }

    private func located(_ entry: LibraryEntry, in package: LibraryHandle<SpielordersPackage>) -> [String]? {
        guard !entry.path.isEmpty else { return nil }
        let location = entry.path.hasPrefix("/") ? URL(fileURLWithPath: entry.path)
            : FileSystemPath(fileSystemRepresentation: spielorders_package_root(package.pointer)).url
                .appending(path: entry.path)
        return location.standardizedFileURL.pathComponents
    }
}
