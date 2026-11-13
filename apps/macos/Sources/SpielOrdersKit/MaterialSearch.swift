import CSpielOrders
import Foundation

struct MaterialHit: Sendable {
    let path: FileSystemPath
    let kind: String
    let modified: String
    let rank: String
    init(path: FileSystemPath, kind: String, modified: String, rank: String) {
        self.path = path
        self.kind = kind
        self.modified = modified
        self.rank = rank
    }
    init?(metadataItem item: NSMetadataItem) {
        let path: FileSystemPath?
        if let reported = item.value(forAttribute: "kMDItemPath") as? String {
            path = try? FileSystemPath(bytes: Array(reported.utf8))
        } else if let url = item.value(forAttribute: NSMetadataItemURLKey) as? URL {
            path = try? FileSystemPath(url: url)
        } else { path = nil }
        guard let path else { return nil }
        let date = item.value(forAttribute: "kMDItemFSContentChangeDate") as? Date
        self.init(path: path,
                  kind: item.value(forAttribute: "kMDItemKind") as? String ?? "",
                  modified: date.map { String(Int64($0.timeIntervalSince1970)) } ?? "",
                  rank: (item.value(forAttribute: "kMDQueryResultContentRelevance") as? NSNumber)?.stringValue ?? "")
    }
}

public struct MaterialBatch: Sendable {
    let provider = "spotlight"
    /// Whether the provider looked at everything in scope, as a `SPIELORDERS_PROVIDER_STATE_*`.
    public let state: Int32
    let hits: [MaterialHit]
    let diagnostics: [String]
    init(state: Int32, hits: [MaterialHit] = [], diagnostics: [String] = []) {
        self.state = state
        self.hits = hits
        self.diagnostics = diagnostics
    }
}

public struct MaterialMatch: Sendable, Identifiable {
    public let id: Int64
    let path: FileSystemPath
    let orderPath: FileSystemPath
    public var url: URL { path.url }
    public let relativePath: String
    public let filename: String
    /// The file's kind in the provider's own words, or "".
    public let kind: String
    public let modified: Date?
}

public struct MaterialAnswer: Sendable {
    /// Read only by the session, which joins it to the Orders for Folder Contents.
    let search: LibraryHandle<SpielordersSearch>
    public let state: Int32
    public let matches: [MaterialMatch]
    public let diagnostics: [String]

    /// The files found in `row`'s Order Directory, most relevant first.
    public func matches(in row: OrderRow) -> [MaterialMatch] {
        matches.filter { $0.orderPath == row.path }
    }
}

extension PackageSession {
    public func materialDirectory() throws -> URL? {
        guard let package else { throw SessionAccessError.reopenRequired }
        let path = spielorders_package_material_directory(package.pointer)
        return path.pointee == 0 ? nil : URL(fileURLWithFileSystemRepresentation: path, isDirectory: true, relativeTo: nil)
    }

    public func materialSearch(_ batch: MaterialBatch, cancellation: LibraryCancellation) throws -> MaterialAnswer {
        guard let package else { throw SessionAccessError.reopenRequired }
        var hits: UnsafeMutablePointer<SpielordersHits>?
        var created: UnsafeMutablePointer<SpielordersSearch>?
        var outcome: UnsafeMutablePointer<SpielordersEntries>?
        defer { spielorders_hits_free(hits) }
        let made = try withInterfaceText(batch.provider) { spielorders_hits_new($0, &hits) }
        _ = try checked(made, nil)
        guard let hits else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        for hit in batch.hits {
            let added = try hit.path.withUnsafeFileSystemRepresentation { path in
                return try withInterfaceText(hit.kind) { kind in
                    try withInterfaceText(hit.modified) { modified in
                        try withInterfaceText(hit.rank) { spielorders_hits_add(hits, path, kind, modified, $0) }
                    }
                }
            }
            _ = try checked(added, nil)
        }
        for diagnostic in batch.diagnostics {
            let added = try withInterfaceText(diagnostic) { spielorders_hits_diagnostic(hits, $0) }
            _ = try checked(added, nil)
        }
        let result = spielorders_package_search_material(package.pointer, hits, batch.state,
                                                         cancellation.token, &created, &outcome)
        let owner = created.map { LibraryHandle($0, free: spielorders_search_free) }
        _ = try checked(result, outcome)
        guard let owner else { throw LibraryFailure(result: SPIELORDERS_RESULT_INTERNAL, entries: []) }
        let search = owner.pointer
        let matches = (0..<spielorders_search_count(search)).map { index in
            let modified = Double(String(cString: spielorders_search_modified(search, index)))
            return MaterialMatch(
                id: index,
                path: FileSystemPath(fileSystemRepresentation: spielorders_search_path(search, index)),
                orderPath: FileSystemPath(fileSystemRepresentation: spielorders_search_order_directory(search, index)),
                relativePath: String(cString: spielorders_search_relative_path(search, index)),
                filename: String(cString: spielorders_search_filename(search, index)),
                kind: String(cString: spielorders_search_kind(search, index)),
                modified: modified.map(Date.init(timeIntervalSince1970:)))
        }
        let diagnostics = (0..<spielorders_search_diagnostic_count(search)).map {
            String(cString: spielorders_search_diagnostic(search, $0))
        }
        return MaterialAnswer(search: owner, state: spielorders_search_state(search), matches: matches,
                              diagnostics: diagnostics)
    }
}

func withInterfaceText<T>(_ text: String, _ body: (UnsafePointer<CChar>) throws -> T) throws -> T {
    guard !text.utf8.contains(0) else { throw LibraryFailure(result: SPIELORDERS_RESULT_BAD_ARGUMENT, entries: []) }
    return try text.withCString(body)
}
