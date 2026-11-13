import Foundation
import SpielOrdersKit
import Testing

@Test func literalCorrection() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let directory = try createOrder(in: root)
    let metadata = directory.appendingPathComponent(".spielorders/order.json")
    let original = try String(contentsOf: metadata, encoding: .utf8)
    let supporting = directory.appendingPathComponent("quote.bin")
    let evidence = Data([0, 255, 12])
    try evidence.write(to: supporting)
    try Data("{broken".utf8).write(to: metadata)
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let document = try await session.expertDocument(directory: directory, artifact: .order)
    #expect(document.text == "{broken")
    _ = try await document.write("still invalid")
    #expect(try String(contentsOf: metadata, encoding: .utf8) == "still invalid")
    #expect(!(try await session.revalidate(directory: directory)).isValid)
    let corrected = try await session.expertDocument(directory: directory, artifact: .order)
    _ = try await corrected.write(original)
    #expect(try await session.revalidate(directory: directory).isValid)
    #expect(try Data(contentsOf: supporting) == evidence)
}

@Test func guardedCorrection() async throws {
    let root = try initializedPackage()
    defer { try? FileManager.default.removeItem(at: root) }
    let session = PackageSession()
    _ = try await session.open(root, cancellation: LibraryCancellation())
    let document = try await session.expertDocument(directory: nil, artifact: .vendors)
    let path = root.appendingPathComponent(".spielorders/vendors.json")
    let external = Data("external edit".utf8)
    try external.write(to: path)
    await #expect(throws: (any Error).self) { try await document.write("overwrite") }
    #expect(try Data(contentsOf: path) == external)
    let target = root.appendingPathComponent("untouched.json")
    try external.write(to: target)
    try FileManager.default.removeItem(at: path)
    try FileManager.default.createSymbolicLink(at: path, withDestinationURL: target)
    await #expect(throws: (any Error).self) { try await session.expertDocument(directory: nil, artifact: .vendors) }
    await #expect(throws: (any Error).self) { try await document.write("overwrite") }
    #expect(try Data(contentsOf: target) == external)
}
