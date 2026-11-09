import Foundation

public final class AuthorizedFolder: @unchecked Sendable {
    public let url: URL
    public init(_ url: URL) throws {
        guard url.isFileURL, url.startAccessingSecurityScopedResource() else {
            throw CocoaError(.fileReadNoPermission)
        }
        self.url = url
    }
    public func bookmark() throws -> Data {
        try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil)
    }
    public static func restore(_ bookmark: Data) throws -> AuthorizedFolder {
        var stale = false
        let url = try URL(resolvingBookmarkData: bookmark, options: [.withSecurityScope, .withoutUI],
                          relativeTo: nil, bookmarkDataIsStale: &stale)
        return try AuthorizedFolder(url)
    }
    deinit { url.stopAccessingSecurityScopedResource() }
}
