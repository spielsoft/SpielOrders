import CSpielOrders
import Darwin
import Foundation
@testable import SpielOrdersKit
import Testing

// Decision sources: Effect and adapter boundaries; Desktop acceptance.
@Test func stoppedProviderHitsReachRustWithoutFrontendEligibilityOrOrderingRules() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root)
    let quote = order.appendingPathComponent("quote.pdf")
    try Data("A quote".utf8).write(to: quote)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let batch = try MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_CANCELLED, hits: [
        MaterialHit(path: FileSystemPath(url: root.appendingPathComponent("README.md")), kind: "", modified: "", rank: ""),
        MaterialHit(path: FileSystemPath(url: quote), kind: "", modified: "", rank: ""),
        MaterialHit(path: FileSystemPath(url: order.appendingPathComponent("README.md")), kind: "", modified: "", rank: ""),
        MaterialHit(path: FileSystemPath(url: quote), kind: "", modified: "", rank: ""),
    ], diagnostics: ["Search stopped."])

    let answer = try await session.materialSearch(batch, cancellation: LibraryCancellation())

    #expect(answer.state == SPIELORDERS_PROVIDER_STATE_CANCELLED)
    #expect(answer.matches.map(\.relativePath) == ["quote.pdf"])
    #expect(answer.diagnostics == ["Search stopped."])
}

@Test func fractionalNativeDatesAllowSearchAndProviderRankOrdersTheMatches() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root)
    var hits: [MaterialHit] = []
    for (name, rank) in [("A-quote.txt", 0.1), ("Z-quote.txt", 0.9)] {
        let file = order.appendingPathComponent(name)
        try Data("A quote".utf8).write(to: file)
        let item = ProviderMetadataItem(attributes: [
            "kMDItemPath": file.path,
            "kMDItemFSContentChangeDate": Date(timeIntervalSince1970: 1_789_000_000.75),
            "kMDQueryResultContentRelevance": NSNumber(value: rank),
        ])
        hits.append(try #require(MaterialHit(metadataItem: item)))
    }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let answer = try await session.materialSearch(
        MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_COMPLETE, hits: hits), cancellation: LibraryCancellation())
    #expect(answer.state == SPIELORDERS_PROVIDER_STATE_COMPLETE)
    #expect(answer.matches.map(\.relativePath) == ["Z-quote.txt", "A-quote.txt"])
}

@Test(arguments: [Array("Caf\u{e9}.txt".utf8), Array("Cafe\u{301}.txt".utf8)])
func spotlightMetadataPreservesReportedPathBytesThroughRust(filename: [UInt8]) async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root)
    let reported = try FileSystemPath(url: order).bytes + [47] + filename
    let path = try FileSystemPath(bytes: reported)
    let descriptor = path.withUnsafeFileSystemRepresentation { Darwin.open($0, O_WRONLY | O_CREAT | O_EXCL, 0o600) }
    try #require(descriptor >= 0)
    Darwin.close(descriptor)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let text = try #require(String(bytes: reported, encoding: .utf8))
    let attributes: [String: Any] = ["kMDItemPath": text, NSMetadataItemURLKey: URL(fileURLWithPath: text)]
    let hit = try #require(MaterialHit(metadataItem: ProviderMetadataItem(attributes: attributes)))
    #expect(hit.path.bytes == reported)
    let answer = try await session.materialSearch(MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_COMPLETE, hits: [hit]),
                                                  cancellation: LibraryCancellation())
    #expect(answer.matches.count == 1)
    #expect(answer.matches.first?.path.bytes == reported)
}

@Test func theCSearchInterfaceAcceptsNonUTF8ProviderPathBytes() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let order = try createOrder(in: root)
    let bytes = try FileSystemPath(url: order).bytes + [47, 255, 254]
    let path = try FileSystemPath(bytes: bytes)
    let hit = MaterialHit(path: path, kind: "", modified: "", rank: "")
    #expect(hit.path.bytes == bytes)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    // APFS cannot create this name; the C path contract still accepts filesystem bytes.
    let answer = try await session.materialSearch(MaterialBatch(state: SPIELORDERS_PROVIDER_STATE_COMPLETE, hits: [hit]),
                                                  cancellation: LibraryCancellation())
    #expect(answer.state == SPIELORDERS_PROVIDER_STATE_COMPLETE)
    #expect(answer.matches.isEmpty)
}

// SDK boundary: metadata attributes are supplied by Spotlight, not by Package rules.
private final class ProviderMetadataItem: NSMetadataItem, @unchecked Sendable {
    private let values: [String: Any]
    init(attributes: [String: Any]) { values = attributes; super.init() }
    override func value(forAttribute key: String) -> Any? { values[key] }
}

private extension FileSystemPath {
    var bytes: [UInt8] {
        withUnsafeFileSystemRepresentation {
            Array(UnsafeBufferPointer(start: UnsafeRawPointer($0).assumingMemoryBound(to: UInt8.self), count: strlen($0)))
        }
    }
}
